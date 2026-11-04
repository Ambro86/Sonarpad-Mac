from pathlib import Path


ROOT = Path(__file__).resolve().parent
MAIN = (ROOT / "src" / "main.rs").read_text(encoding="utf-8")


def test_tv_guide_exposes_yesterday_through_five_days_after_today():
    assert "const TV_GUIDE_FIRST_DAY_OFFSET: i64 = -1;" in MAIN
    assert "const TV_GUIDE_LAST_DAY_OFFSET: i64 = 5;" in MAIN
    assert "(TV_GUIDE_FIRST_DAY_OFFSET..=TV_GUIDE_LAST_DAY_OFFSET)" in MAIN


def test_tv_guide_day_labels_are_always_italian():
    helper_start = MAIN.index("fn tv_guide_day_labels_for_date")
    helper_end = MAIN.index("fn italian_month_name", helper_start)
    helper = MAIN[helper_start:helper_end]

    assert '"Ieri".to_string()' in helper
    assert '"Oggi".to_string()' in helper
    assert '"Domani".to_string()' in helper
    assert "italian_month_name(date.month())" in helper
    assert "Yesterday" not in helper
    assert "Today" not in helper
    assert "Tomorrow" not in helper


def test_tv_guide_regression_example_reaches_august_29():
    assert '("29 agosto 2026".to_string(), 5)' in MAIN
