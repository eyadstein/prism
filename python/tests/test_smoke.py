from prism_py import visible_range_nm


def test_visible_range_is_ordered() -> None:
    low, high = visible_range_nm()
    assert low < high


def test_visible_range_matches_engine() -> None:
    assert visible_range_nm() == (380.0, 780.0)
