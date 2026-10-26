//! Longwave history joins the ordinary durable log, so every existing log query,
//! map and worked-before lookup sees it. Sync is additive, never a remote delete.
//! Uploads claim their stored row durably before the non-idempotent POST: an
//! interrupted/uncertain send is visible and is never automatically repeated.

use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use propagation::longwave::Failure;
use tauri::State;
use tempo_app::dto::{ImportStats, LoggedQso, UploadReportDto};
use tempo_core::logbook::{OpClass, QsoRecord, UploadOutcome, UploadStatus};

use crate::{engine_lock, SharedEngine};

// Serialize this process's reads and sends without holding the radio's Engine lock.
// The log writer's optimistic row check also arbitrates claims from other instances.
static OPERATIONS: Mutex<()> = Mutex::new(());
static CONFIGURATION_GENERATION: AtomicU64 = AtomicU64::new(0);
static CREDENTIAL_WRITES: AtomicUsize = AtomicUsize::new(0);

pub(super) fn configuration_changed() {
    CONFIGURATION_GENERATION.fetch_add(1, Ordering::SeqCst);
}

/// No credential bytes cross this guard. Even a failed replacement invalidates
/// a client that was prepared before the operator tried to replace or forget it.
pub(super) struct CredentialChange;

pub(super) fn credential_change() -> CredentialChange {
    CREDENTIAL_WRITES.fetch_add(1, Ordering::SeqCst);
    configuration_changed();
    CredentialChange
}

impl Drop for CredentialChange {
    fn drop(&mut self) {
        configuration_changed();
        CREDENTIAL_WRITES.fetch_sub(1, Ordering::SeqCst);
    }
}

fn current(generation: u64) -> bool {
    CREDENTIAL_WRITES.load(Ordering::SeqCst) == 0
        && CONFIGURATION_GENERATION.load(Ordering::SeqCst) == generation
}

fn selected(engine: &SharedEngine) -> (String, String, String) {
    let e = engine_lock(engine);
    let s = e.settings();
    (
        s.longwave_url.trim().to_owned(),
        s.longwave_logbook_id.trim().to_owned(),
        s.mycall.clone(),
    )
}

fn client(url: &str) -> Result<propagation::longwave::Longwave, Failure> {
    let token = crate::longwave_keychain()
        .map_err(|_| Failure::Configuration)?
        .get_password()
        .map_err(|_| Failure::Configuration)?;
    crate::longwave_trust::client(url, &token)
}

#[tauri::command]
pub async fn longwave_sync_logbook(state: State<'_, SharedEngine>) -> Result<ImportStats, String> {
    let engine = Arc::clone(&state);
    tauri::async_runtime::spawn_blocking(move || {
        let _operation = OPERATIONS.lock().unwrap_or_else(|e| e.into_inner());
        let generation = CONFIGURATION_GENERATION.load(Ordering::SeqCst);
        let (url, book, _) = selected(&engine);
        if url.is_empty() || book.is_empty() {
            return Err("Save a Longwave server and selected logbook in Settings first".into());
        }
        let remote = client(&url).and_then(|c| c.contacts(&book));
        let rows = match remote {
            Ok(rows) => rows,
            Err(error) => {
                crate::conn_log("Longwave", "error", format!("History sync: {error}"));
                return Err(error.to_string());
            }
        };
        let mut adif = tempo_core::logbook::adif_header();
        for row in rows {
            let text = row.adif_record().map_err(|e| e.to_string())?;
            let mut parsed = tempo_core::logbook::parse_adif(&text);
            if parsed.len() != 1 {
                return Err(Failure::InvalidResponse.to_string());
            }
            let mut record = parsed.remove(0);
            record.upload.longwave = Some(UploadStatus {
                outcome: UploadOutcome::Accepted,
                when_unix: 0,
                detail: None,
            });
            adif.push_str(&tempo_core::logbook::adif_record_own_log(&record));
        }
        if !current(generation) {
            return Err("Longwave settings or token changed during sync; sync again".into());
        }
        merge_history_at(&engine, &url, &book, &adif, generation)
    })
    .await
    .map_err(|_| "Longwave history sync could not complete".to_string())?
}

#[cfg(test)]
fn merge_history(
    engine: &SharedEngine,
    url: &str,
    book: &str,
    adif: &str,
) -> Result<ImportStats, String> {
    merge_history_at(
        engine,
        url,
        book,
        adif,
        CONFIGURATION_GENERATION.load(Ordering::SeqCst),
    )
}

fn merge_history_at(
    engine: &SharedEngine,
    url: &str,
    book: &str,
    adif: &str,
    generation: u64,
) -> Result<ImportStats, String> {
    let run = tempo_app::loghistory::new_run();
    let (made, durability) = tempo_app::logwrite::until_written(|| {
        tempo_app::logwrite::import_adif_in_run_checked(engine, adif, &run, |e| {
            let s = e.settings();
            if !current(generation)
                || s.longwave_url.trim() != url
                || s.longwave_logbook_id.trim() != book
            {
                return Err("Longwave settings or token changed during sync; sync again".into());
            }
            Ok(())
        })
    });
    let (added, skipped, updated, total) = made?;
    durability
        .wait(tempo_app::logstore::DURABLE_WAIT)
        .map_err(crate::durability_failed)?;
    crate::conn_log(
        "Longwave",
        "ok",
        format!("History synced: {added} added, {updated} updated, {skipped} already present"),
    );
    Ok(ImportStats {
        added,
        skipped,
        updated,
        total,
        run: Some(run),
    })
}

fn status(outcome: UploadOutcome) -> UploadStatus {
    UploadStatus {
        outcome,
        when_unix: crate::now_unix(),
        detail: None,
    }
}

/// Only the persisted row is sent. Incoming UI upload flags are not authority.
fn claim(
    engine: &SharedEngine,
    dto: &LoggedQso,
) -> Result<(Arc<QsoRecord>, UploadStatus), Failure> {
    let id = dto
        .id
        .as_deref()
        .and_then(|id| id.parse().ok())
        .ok_or(Failure::Configuration)?;
    let expected = crate::upload_record(dto.clone());
    let pending = status(UploadOutcome::Pending);
    let (made, durability) = tempo_app::logwrite::until_written(|| {
        tempo_app::logwrite::change_row(
            engine,
            id,
            "Longwave upload claim",
            |_, row| {
                if !tempo_core::logbook::same_push_key(row, &expected)
                    || row
                        .upload
                        .longwave
                        .as_ref()
                        .is_some_and(|s| s.outcome.is_sent())
                {
                    return Ok(None);
                }
                let mut after = (**row).clone();
                after.upload.longwave = Some(pending.clone());
                Ok(Some((OpClass::Stamp, Some(after))))
            },
            |_, (before, _)| before,
        )
    });
    let row = made
        .map_err(|_| Failure::Configuration)?
        .map_err(|_| Failure::Configuration)?
        .ok_or(Failure::Refused)?;
    durability
        .wait(tempo_app::logstore::DURABLE_WAIT)
        .map_err(|_| Failure::Configuration)?;
    Ok((row, pending))
}

pub(super) fn push(dto: &LoggedQso, engine: &SharedEngine) -> Result<String, Failure> {
    let _operation = OPERATIONS.lock().unwrap_or_else(|e| e.into_inner());
    let generation = CONFIGURATION_GENERATION.load(Ordering::SeqCst);
    let (url, book, call) = selected(engine);
    if url.is_empty() || book.is_empty() {
        return Err(Failure::Configuration);
    }
    let client = client(&url)?;
    send_claimed_checked(
        dto,
        engine,
        &book,
        &call,
        |contact| client.upload(contact),
        || current(generation) && selected(engine) == (url.clone(), book.clone(), call.clone()),
    )
}

#[cfg(test)]
fn send_claimed(
    dto: &LoggedQso,
    engine: &SharedEngine,
    book: &str,
    call: &str,
    send: impl FnOnce(&propagation::longwave::Contact) -> Result<String, Failure>,
) -> Result<String, Failure> {
    send_claimed_checked(dto, engine, book, call, send, || true)
}

fn send_claimed_checked(
    dto: &LoggedQso,
    engine: &SharedEngine,
    book: &str,
    call: &str,
    send: impl FnOnce(&propagation::longwave::Contact) -> Result<String, Failure>,
    still_current: impl FnOnce() -> bool,
) -> Result<String, Failure> {
    // Validate the form before claiming it. The actual payload below comes from storage.
    crate::longwave_contact(book, call, crate::upload_record(dto.clone()))?;
    let (row, pending) = claim(engine, dto)?;
    let contact = crate::longwave_contact(
        book,
        call,
        crate::upload_record(LoggedQso::from((*row).clone())),
    );
    let result = contact.and_then(|contact| {
        if !still_current() {
            return Err(Failure::Configuration);
        }
        send(&contact)
    });
    let outcome = match &result {
        Ok(_) => UploadOutcome::Accepted,
        Err(Failure::UnconfirmedUpload) => UploadOutcome::Pending,
        Err(Failure::Credentials) => UploadOutcome::AuthFail,
        Err(_) => UploadOutcome::Rejected,
    };
    let id = row.id.ok_or(Failure::Configuration)?;
    let (made, durability) = tempo_app::logwrite::until_written(|| {
        tempo_app::logwrite::change_row(
            engine,
            id,
            "Longwave upload result",
            |_, stored| {
                if stored.upload.longwave.as_ref() != Some(&pending) {
                    return Ok(None);
                }
                let mut after = (**stored).clone();
                after.upload.longwave = Some(status(outcome));
                Ok(Some((OpClass::Stamp, Some(after))))
            },
            |_, _| (),
        )
    });
    // Only our Pending claim can be finalized. A receipt established meanwhile
    // by another instance's history sync must never be replaced by a refusal.
    if made.is_err()
        || made.as_ref().is_ok_and(|r| r.is_err())
        || durability.wait(tempo_app::logstore::DURABLE_WAIT).is_err()
    {
        return Err(Failure::UnconfirmedUpload);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempo_app::engine::Engine;
    use tempo_app::test_util::StoredLog;

    fn fixture() -> (SharedEngine, LoggedQso) {
        let mut e = Engine::new("N0CALL", "FN31", 0);
        let row = tempo_core::logbook::parse_adif(
            "<CALL:4>W1AW<STATION_CALLSIGN:6>N0CALL<BAND:3>20m<MODE:2>CW<FREQ:7>14.0475<QSO_DATE:8>20261025<TIME_ON:6>120000<EOR>",
        ).remove(0);
        e.log_qso(row);
        let dto = LoggedQso::from((*e.stored_log()[0]).clone());
        (Arc::new(Mutex::new(e)), dto)
    }

    #[test]
    fn longwave_claim_is_visible_before_send_and_success_survives_dto_refresh() {
        let (engine, dto) = fixture();
        let result = send_claimed(&dto, &engine, "book", "N0CALL", |_| {
            assert_eq!(
                engine.stored_log()[0]
                    .upload
                    .longwave
                    .as_ref()
                    .unwrap()
                    .outcome,
                UploadOutcome::Pending
            );
            Ok("remote-id".into())
        });
        assert_eq!(result, Ok("remote-id".into()));
        let stored = engine.stored_log();
        assert_eq!(
            stored[0].upload.longwave.as_ref().unwrap().outcome,
            UploadOutcome::Accepted
        );
        let mut stale = dto;
        stale.upload.longwave = None;
        assert_eq!(
            send_claimed(&stale, &engine, "book", "N0CALL", |_| panic!(
                "duplicate POST"
            )),
            Err(Failure::Refused)
        );
    }

    #[test]
    fn longwave_uncertain_send_is_never_repeated_but_definite_refusal_can_be_retried() {
        for (failure, expected, retry) in [
            (Failure::UnconfirmedUpload, UploadOutcome::Pending, false),
            (Failure::Credentials, UploadOutcome::AuthFail, true),
            (Failure::Refused, UploadOutcome::Rejected, true),
        ] {
            let (engine, dto) = fixture();
            assert_eq!(
                send_claimed(&dto, &engine, "book", "N0CALL", |_| Err(failure)),
                Err(failure)
            );
            assert_eq!(
                engine.stored_log()[0]
                    .upload
                    .longwave
                    .as_ref()
                    .unwrap()
                    .outcome,
                expected
            );
            let sends = std::cell::Cell::new(0);
            let result = send_claimed(&dto, &engine, "book", "N0CALL", |_| {
                sends.set(sends.get() + 1);
                Ok("receipt".into())
            });
            assert_eq!(sends.get(), usize::from(retry));
            assert_eq!(result.is_ok(), retry);
        }
    }

    #[test]
    fn longwave_invalid_or_deleted_contact_never_reaches_transport() {
        let (engine, mut dto) = fixture();
        dto.time_known = false;
        assert!(send_claimed(&dto, &engine, "book", "N0CALL", |_| panic!("invalid POST")).is_err());
        assert!(engine.stored_log()[0].upload.longwave.is_none());
        dto.time_known = true;
        dto.id = None;
        assert!(send_claimed(&dto, &engine, "book", "N0CALL", |_| panic!("unnamed POST")).is_err());
    }

    #[test]
    fn longwave_two_simultaneous_pushes_make_only_one_post() {
        let (engine, dto) = fixture();
        let barrier = Arc::new(std::sync::Barrier::new(2));
        let sends = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let results = std::thread::scope(|scope| {
            let mut joins = Vec::new();
            for _ in 0..2 {
                joins.push(scope.spawn(|| {
                    barrier.wait();
                    send_claimed(&dto, &engine, "book", "N0CALL", |_| {
                        sends.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                        Ok("receipt".into())
                    })
                }));
            }
            joins
                .into_iter()
                .map(|j| j.join().unwrap())
                .collect::<Vec<_>>()
        });
        assert_eq!(sends.load(std::sync::atomic::Ordering::SeqCst), 1);
        assert_eq!(results.iter().filter(|r| r.is_ok()).count(), 1);
    }

    #[test]
    fn longwave_changed_destination_after_claim_cancels_before_transport() {
        let (engine, dto) = fixture();
        let result = send_claimed_checked(
            &dto,
            &engine,
            "book",
            "N0CALL",
            |_| panic!("superseded destination must not receive a POST"),
            || {
                assert_eq!(
                    engine.stored_log()[0]
                        .upload
                        .longwave
                        .as_ref()
                        .unwrap()
                        .outcome,
                    UploadOutcome::Pending
                );
                false
            },
        );
        assert_eq!(result, Err(Failure::Configuration));
        assert_eq!(
            engine.stored_log()[0]
                .upload
                .longwave
                .as_ref()
                .unwrap()
                .outcome,
            UploadOutcome::Rejected
        );
    }

    #[test]
    fn longwave_cancelled_claim_preserves_a_receipt_established_by_another_sync() {
        let (engine, dto) = fixture();
        let result = send_claimed_checked(
            &dto,
            &engine,
            "book",
            "N0CALL",
            |_| panic!("cancelled claim must not POST"),
            || {
                let row = engine.stored_log()[0].clone();
                let (stamped, durability) = tempo_app::logwrite::stamp_push(
                    &engine,
                    &row,
                    tempo_core::logbook::UploadService::Longwave,
                    status(UploadOutcome::Accepted),
                );
                assert!(stamped);
                durability.wait(tempo_app::logstore::DURABLE_WAIT).unwrap();
                false
            },
        );
        assert_eq!(result, Err(Failure::Configuration));
        assert_eq!(
            engine.stored_log()[0]
                .upload
                .longwave
                .as_ref()
                .unwrap()
                .outcome,
            UploadOutcome::Accepted
        );
        assert_eq!(
            send_claimed(&dto, &engine, "book", "N0CALL", |_| panic!(
                "receipt must block retry"
            )),
            Err(Failure::Refused)
        );
    }

    #[test]
    fn longwave_credential_replacement_invalidates_prepared_clients_without_reading_a_secret() {
        let old = CONFIGURATION_GENERATION.load(Ordering::SeqCst);
        let change = credential_change();
        assert!(!current(old));
        let during = CONFIGURATION_GENERATION.load(Ordering::SeqCst);
        assert!(
            !current(during),
            "a write in flight cannot supply a stable credential"
        );
        drop(change);
        assert!(!current(old));
        assert!(!current(during));
        assert!(current(CONFIGURATION_GENERATION.load(Ordering::SeqCst)));
    }

    #[test]
    fn longwave_history_joins_shared_log_without_reuploading_or_erasing_local_contacts() {
        let (engine, _) = fixture();
        {
            let mut e = engine_lock(&engine);
            let mut settings = e.settings().clone();
            settings.longwave_url = "https://log.example.invalid".into();
            settings.longwave_logbook_id = "book".into();
            e.apply_settings(settings);
            let _ = e.take_pending_uploads();
        }
        let mut remote = tempo_core::logbook::parse_adif(
            "<CALL:5>W2NEW<STATION_CALLSIGN:6>N0CALL<BAND:3>40m<MODE:3>FT8<FREQ:5>7.074<QSO_DATE:8>20261024<TIME_ON:6>143000<GRIDSQUARE:4>FN32<EOR>"
        ).remove(0);
        remote.upload.longwave = Some(UploadStatus {
            outcome: UploadOutcome::Accepted,
            when_unix: 0,
            detail: None,
        });
        let tallies = crate::LogTallies::default();
        assert_eq!(
            crate::log_folds::log_stats(&engine, &tallies)
                .unwrap()
                .total,
            1
        );
        let queries = crate::log_queries::LogQueries::default();
        let ask = |q| {
            queries
                .answer(&engine, &q, &|_| None, &|_| None, &|_| None)
                .unwrap()
        };
        let grids = ask(crate::log_queries::LogQuestion::WorkedGrids {});
        assert!(!grids.as_array().unwrap().iter().any(|g| g == "FN32"));
        let text =
            tempo_core::logbook::adif_header() + &tempo_core::logbook::adif_record_own_log(&remote);
        let first = merge_history(&engine, "https://log.example.invalid", "book", &text).unwrap();
        assert_eq!((first.added, first.total), (1, 2));
        assert_eq!(
            crate::log_folds::log_stats(&engine, &tallies)
                .unwrap()
                .total,
            2
        );
        assert_eq!(
            crate::log_folds::journey_kept(&engine, &tallies)
                .unwrap()
                .summary(crate::now_unix())
                .total_qsos,
            2
        );
        let grids = ask(crate::log_queries::LogQuestion::WorkedGrids {});
        assert!(
            grids.as_array().unwrap().iter().any(|g| g == "FN32"),
            "the existing map's query sees downloaded grids"
        );
        let history = ask(crate::log_queries::LogQuestion::CallHistory {
            call: "W2NEW".into(),
            band: "40m".into(),
            mode: "FT8".into(),
            match_mode: true,
        });
        assert_eq!(
            history["count"], 1,
            "the existing call-history panel sees the imported contact"
        );
        assert!(first.run.is_some());
        let second = merge_history(&engine, "https://log.example.invalid", "book", &text).unwrap();
        assert_eq!((second.added, second.updated, second.total), (0, 0, 2));
        let empty = merge_history(
            &engine,
            "https://log.example.invalid",
            "book",
            &tempo_core::logbook::adif_header(),
        )
        .unwrap();
        assert_eq!(
            empty.total, 2,
            "an empty remote log never deletes local history"
        );
        assert!(
            engine_lock(&engine).take_pending_uploads().is_empty(),
            "history import must not POST old contacts"
        );
        let rows = engine.stored_log();
        assert!(rows.iter().any(|r| r.call == "W1AW"));
        assert!(rows.iter().any(|r| r.call == "W2NEW"
            && r.grid.as_deref() == Some("FN32")
            && r.upload.longwave.is_some()));
        assert!(merge_history(&engine, "https://other.example.invalid", "book", &text).is_err());
        assert!(
            merge_history(&engine, "https://log.example.invalid", "other-book", &text).is_err()
        );
        assert_eq!(engine.stored_log().len(), 2);
    }
}

#[tauri::command]
pub async fn longwave_push_qso(
    record: LoggedQso,
    state: State<'_, SharedEngine>,
) -> Result<UploadReportDto, String> {
    let engine = Arc::clone(&state);
    tauri::async_runtime::spawn_blocking(move || {
        let result = push(&record, &engine);
        let (ok, detail) = match &result {
            Ok(_) => (true, crate::longwave_saved_detail()),
            Err(error) => (false, crate::longwave_stamp(*error)),
        };
        crate::note_conn_health("longwave", ok, detail);
        crate::conn_log("Longwave", if ok { "ok" } else { "error" },
            if ok { "Contact saved in Longwave".to_owned() }
            else { result.as_ref().unwrap_err().to_string() });
        result.map(|_| UploadReportDto {
            dispatched: 1, outcome: "accepted".into(), detail: None,
            skipped_edited: 0, skipped_deleted: 0, held_mode: 0,
        }).map_err(|error| {
            if error == Failure::Refused {
                "Longwave upload refused or already sent/unconfirmed; sync history and check Longwave before retrying".into()
            } else { error.to_string() }
        })
    }).await.map_err(|_| "Longwave upload could not complete".to_owned())?
}
