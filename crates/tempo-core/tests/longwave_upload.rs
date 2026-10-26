//! Longwave receipts use the ordinary upload stamp, never QSL confirmation state.

use tempo_core::logbook::sqlite::{LogDb, Resolved};
use tempo_core::logbook::{
    adif_record_own_log, parse_adif, LogOp, Logbook, QsoRecord, UploadDetail, UploadOutcome,
    UploadService, UploadState, UploadStatus,
};

const CONTACT: &str = "<CALL:4>W1AW<BAND:3>20m<MODE:3>FT8<QSO_DATE:8>20261022\
                       <TIME_ON:6>120000<EOR>";

fn record() -> QsoRecord {
    parse_adif(CONTACT).remove(0)
}

fn status(outcome: UploadOutcome, when_unix: i64) -> UploadStatus {
    UploadStatus {
        outcome,
        when_unix,
        detail: None,
    }
}

fn unconfirmed(row: &QsoRecord) {
    assert!(!row.confirmed);
    assert!(!row.award_confirmed);
    assert!(!row.qsl_rcvd.any());
    assert!(!row.qsl_sent.sent);
}

#[test]
fn longwave_old_records_default_to_none_and_sqlite_round_trips_all_outcomes() {
    let mut book = Logbook::new();
    book.add(record());
    let old = QsoRecord::clone(&book.records()[0]);
    assert_eq!(old.upload.longwave, None);
    assert!(!adif_record_own_log(&old).contains("APP_TEMPO_UL_LONGWAVE"));
    let mut db = LogDb::open_in_memory().unwrap();
    db.insert_all([(&old, Resolved::default())]).unwrap();
    assert_eq!(db.load_all().unwrap()[0].upload.longwave, None);

    for outcome in [
        UploadOutcome::Pending,
        UploadOutcome::Accepted,
        UploadOutcome::Duplicate,
        UploadOutcome::Rejected,
        UploadOutcome::AuthFail,
    ] {
        let mut row = old.clone();
        row.upload.longwave = Some(UploadStatus {
            detail: Some(UploadDetail::RecordRefused),
            ..status(outcome, 1_790_000_000)
        });
        let text = adif_record_own_log(&row);
        assert!(text.contains("APP_TEMPO_UL_LONGWAVE"));
        let parsed = parse_adif(&text).remove(0);
        assert_eq!(parsed.upload.longwave, row.upload.longwave);
        assert!(!parsed
            .extra
            .iter()
            .any(|(k, _)| k == "APP_TEMPO_UL_LONGWAVE"));
        unconfirmed(&parsed);

        let mut db = LogDb::open_in_memory().unwrap();
        db.insert_all([(&parsed, Resolved::default())]).unwrap();
        let back = db.load_all().unwrap().remove(0);
        assert_eq!(back.upload, row.upload);
        assert_eq!(adif_record_own_log(&back), text);
        unconfirmed(&back);
    }
}

#[test]
fn longwave_stamp_targets_the_named_contact_and_reads_health_without_confirming() {
    let mut book = Logbook::new();
    let id = book.add(record());
    let pushed = QsoRecord::clone(&book.records()[0]);
    let mut twin = record();
    twin.when_unix += 3600;
    book.add(twin);

    let pending = status(UploadOutcome::Pending, 100);
    let effect = book.apply(LogOp::Stamp {
        id,
        service: UploadService::Longwave,
        status: pending.clone(),
    });
    assert_eq!(effect.changed, vec![id]);
    assert_eq!(book.records()[0].upload.longwave, Some(pending));
    assert_eq!(book.records()[1].upload.longwave, None);
    assert_eq!(book.upload_health().longwave.last_success_unix, None);
    assert_eq!(book.upload_health().longwave.last_failure_unix, None);
    unconfirmed(&book.records()[0]);

    let accepted = status(UploadOutcome::Accepted, 200);
    assert!(book.stamp_longwave_upload(&pushed, accepted.clone()));
    assert_eq!(book.records()[0].upload.longwave, Some(accepted));
    assert_eq!(book.records()[1].upload.longwave, None);
    let mut health = book.upload_health();
    assert_eq!(health.longwave.last_success_unix, Some(200));
    assert_eq!(health.longwave.last_failure_unix, None);
    assert_eq!(health.qrz.last_success_unix, None);
    unconfirmed(&book.records()[0]);

    let failure = UploadStatus {
        detail: Some(UploadDetail::Credentials),
        ..status(UploadOutcome::AuthFail, 300)
    };
    assert!(book.stamp_longwave_upload(&pushed, failure));
    health.add(&book.records()[0]);
    assert_eq!(health.longwave.last_success_unix, Some(200));
    assert_eq!(health.longwave.last_failure_unix, Some(300));
    assert_eq!(
        health.longwave.last_failure_detail,
        Some(UploadDetail::Credentials)
    );
    unconfirmed(&book.records()[0]);
}

#[test]
fn longwave_import_upgrades_matching_local_row_without_qsl_confirmation() {
    let mut book = Logbook::new();
    let id = book.add(record());
    let pushed = QsoRecord::clone(&book.records()[0]);
    let pending = status(UploadOutcome::Pending, 100);
    assert!(book.stamp_longwave_upload(&pushed, pending));
    let mut remote = record();
    remote.upload.longwave = Some(status(UploadOutcome::Accepted, 200));
    let (added, skipped, merged) = book.import_adif(&adif_record_own_log(&remote));
    assert!(added.is_empty());
    assert_eq!((merged, skipped), (1, 1));
    assert_eq!(book.len(), 1);
    assert_eq!(book.records()[0].id, Some(id));
    assert_eq!(book.records()[0].upload.longwave, remote.upload.longwave);
    unconfirmed(&book.records()[0]);

    remote.upload.longwave = Some(status(UploadOutcome::Pending, 50));
    book.import_adif(&adif_record_own_log(&remote));
    assert_eq!(
        book.records()[0].upload.longwave,
        Some(status(UploadOutcome::Accepted, 200))
    );
    book.import_adif(CONTACT);
    assert_eq!(
        book.records()[0].upload.longwave,
        Some(status(UploadOutcome::Accepted, 200))
    );
    unconfirmed(&book.records()[0]);
}

#[test]
fn longwave_merge_is_per_service_and_keeps_newer_stamps() {
    let mut local = UploadState {
        qrz: Some(status(UploadOutcome::Accepted, 400)),
        longwave: Some(status(UploadOutcome::Pending, 100)),
        ..Default::default()
    };
    local.merge_recent(&UploadState {
        longwave: Some(status(UploadOutcome::Accepted, 200)),
        ..Default::default()
    });
    local.merge_recent(&UploadState {
        longwave: Some(status(UploadOutcome::Rejected, 50)),
        ..Default::default()
    });
    assert_eq!(local.longwave, Some(status(UploadOutcome::Accepted, 200)));
    assert_eq!(local.qrz, Some(status(UploadOutcome::Accepted, 400)));
}

#[test]
fn longwave_history_receipt_promotes_pending_even_with_an_older_or_unknown_time() {
    for receipt_time in [100, 0] {
        let mut book = Logbook::new();
        let id = book.add(record());
        let pushed = QsoRecord::clone(&book.records()[0]);
        assert!(book.stamp_longwave_upload(&pushed, status(UploadOutcome::Pending, 200)));
        let mut remote = record();
        remote.upload.longwave = Some(status(UploadOutcome::Accepted, receipt_time));
        let (added, skipped, merged) = book.import_adif(&adif_record_own_log(&remote));
        assert!(added.is_empty());
        assert_eq!((skipped, merged), (1, 1));
        assert_eq!(book.records()[0].id, Some(id));
        assert_eq!(book.records()[0].upload.longwave, remote.upload.longwave);
        unconfirmed(&book.records()[0]);

        let accepted = book.records()[0].clone();
        for outcome in [
            UploadOutcome::Accepted,
            UploadOutcome::Pending,
            UploadOutcome::Rejected,
            UploadOutcome::AuthFail,
            UploadOutcome::Duplicate,
        ] {
            for when in [0, 50, 100, 200, 300] {
                remote.upload.longwave = Some(status(outcome, when));
                let (added, skipped, merged) = book.import_adif(&adif_record_own_log(&remote));
                assert!(added.is_empty());
                assert_eq!((skipped, merged), (1, 0), "{outcome:?}@{when}");
                assert_eq!(book.records()[0], accepted);
                assert!(std::sync::Arc::ptr_eq(&book.records()[0], &accepted));
            }
        }
        let (added, skipped, merged) = book.import_adif(CONTACT);
        assert!(added.is_empty());
        assert_eq!((skipped, merged), (1, 0));
        assert_eq!(book.records()[0], accepted);
    }
}

#[test]
fn longwave_receipt_precedence_does_not_change_other_services_timestamp_rule() {
    for (local_outcome, incoming_outcome, local_time, incoming_time, expected) in [
        (
            UploadOutcome::Pending,
            UploadOutcome::Accepted,
            200,
            100,
            UploadOutcome::Pending,
        ),
        (
            UploadOutcome::Accepted,
            UploadOutcome::Pending,
            100,
            200,
            UploadOutcome::Pending,
        ),
        (
            UploadOutcome::Accepted,
            UploadOutcome::Rejected,
            100,
            200,
            UploadOutcome::Rejected,
        ),
    ] {
        let mut local = UploadState {
            lotw: Some(status(local_outcome, local_time)),
            eqsl: Some(status(local_outcome, local_time)),
            qrz: Some(status(local_outcome, local_time)),
            clublog: Some(status(local_outcome, local_time)),
            longwave: None,
        };
        local.merge_recent(&UploadState {
            lotw: Some(status(incoming_outcome, incoming_time)),
            eqsl: Some(status(incoming_outcome, incoming_time)),
            qrz: Some(status(incoming_outcome, incoming_time)),
            clublog: Some(status(incoming_outcome, incoming_time)),
            longwave: None,
        });
        for slot in [&local.lotw, &local.eqsl, &local.qrz, &local.clublog] {
            assert_eq!(slot.as_ref().unwrap().outcome, expected);
            assert_eq!(
                slot.as_ref().unwrap().when_unix,
                local_time.max(incoming_time)
            );
        }
    }
}
