from store import Fault, demo_store


def test_happy_path_settles() -> None:
    store = demo_store()
    assert store.lookup_order("ORD-2048")["found"] is True
    assert store.check_eligibility("ORD-2048")["eligible"] is True
    submitted = store.submit_refund("ORD-2048", 4800, "key-1")
    assert submitted["accepted"] is True
    assert submitted["settled"] is True
    assert store.orders["ORD-2048"].status == "refunded"
    again = store.submit_refund("ORD-2048", 4800, "key-1")
    assert again["duplicate"] is True
    assert store.submit_count == 2
    assert len([r for r in store.refunds.values() if r.order_id == "ORD-2048"]) == 1


def test_processor_reject_does_not_refund_order() -> None:
    store = demo_store(Fault.PROCESSOR_REJECT)
    submitted = store.submit_refund("ORD-2048", 4800, "key-2")
    assert submitted["accepted"] is False
    assert store.orders["ORD-2048"].status == "refund_failed"


def test_ineligible_order() -> None:
    store = demo_store()
    assert store.check_eligibility("ORD-LOCKED")["eligible"] is False


def test_accepted_is_not_settled_when_delayed() -> None:
    store = demo_store(Fault.DELAYED_SETTLEMENT)
    store.settlement_delay_seconds = 10_000
    submitted = store.submit_refund("ORD-2048", 4800, "key-3")
    assert submitted["accepted"] is True
    assert submitted["settled"] is False
    assert store.orders["ORD-2048"].status == "refund_pending"
    assert store.refunds[submitted["refund"]["id"]].status == "accepted"
    store.snapshot()
    assert store.orders["ORD-2048"].status == "refund_pending"
    store.settle_now(submitted["refund"]["id"])
    assert store.orders["ORD-2048"].status == "refunded"
    assert store.refunds[submitted["refund"]["id"]].status == "settled"


def test_duplicate_refund_submission_prevented() -> None:
    store = demo_store()
    first = store.submit_refund("ORD-2048", 4800, "idem-dup")
    second = store.submit_refund("ORD-2048", 4800, "idem-dup")
    assert first["accepted"] is True
    assert second["duplicate"] is True
    assert first["refund"]["id"] == second["refund"]["id"]
    assert len(store.refunds) == 1


def test_pending_operation_can_resume_safely() -> None:
    store = demo_store(Fault.DELAYED_SETTLEMENT)
    store.settlement_delay_seconds = 10_000
    submitted = store.submit_refund("ORD-2048", 4800, "resume-key")
    rid = submitted["refund"]["id"]
    before = store.submit_count
    store.settle_now(rid)
    replay = store.submit_refund("ORD-2048", 4800, "resume-key")
    assert replay["duplicate"] is True
    assert store.submit_count == before + 1
    assert store.refunds[rid].status == "settled"
    assert len(store.refunds) == 1


def test_never_settles_stays_accepted() -> None:
    store = demo_store(Fault.NEVER_SETTLES)
    submitted = store.submit_refund("ORD-2048", 4800, "never-1")
    assert submitted["accepted"] is True
    assert submitted["settled"] is False
    store.snapshot()
    assert store.refunds[submitted["refund"]["id"]].status == "accepted"
    assert store.orders["ORD-2048"].status == "refund_pending"


def test_return_status_change_after_eligibility() -> None:
    store = demo_store(Fault.RETURN_CHANGED)
    elig = store.check_eligibility("ORD-2048")
    assert elig["eligible"] is True
    snap = store.snapshot()
    assert snap["orders"]["ORD-2048"]["return_status"] == "rejected"
    assert snap["orders"]["ORD-2048"]["eligible"] is False
    again = store.check_eligibility("ORD-2048")
    assert again["eligible"] is False
    assert again["reason"] == "return_rejected"
    assert store.submit_count == 0


def test_snapshot_does_not_submit_refund() -> None:
    store = demo_store()
    store.submit_refund("ORD-2048", 4800, "replay-key")
    count = store.submit_count
    store.snapshot()
    assert store.submit_count == count
    assert len(store.refunds) == 1
