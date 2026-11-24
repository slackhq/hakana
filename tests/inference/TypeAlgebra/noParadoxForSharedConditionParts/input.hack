interface Val {}
final class IntVal implements Val {
    public function __construct(public int $value) {}
}
final class FloatVal implements Val {
    public function __construct(public float $value) {}
}

function compare(Val $a, Val $b): int {
    if ($a is IntVal && $b is IntVal) {
        return $a->value <=> $b->value;
    }

    if ($a is FloatVal && $b is FloatVal) {
        return $a->value <=> $b->value;
    }

    if (($a is IntVal || $a is FloatVal) && ($b is IntVal || $b is FloatVal)) {
        $av = $a is IntVal ? (float)$a->value : $a->value;
        $bv = $b is IntVal ? (float)$b->value : $b->value;
        return $av <=> $bv;
    }

    return 0;
}

function both_gates(bool $first_on, bool $second_on, bool $is_first, bool $is_second): string {
    if (!$first_on && !$second_on) {
        return 'off';
    }

    if (!($is_first && $first_on) && !($is_second && $second_on)) {
        return 'hidden';
    }

    return 'shown';
}

function only_merged_away(bool $a_missing, bool $a_merged, bool $b_missing, bool $b_merged): bool {
    if ($a_missing || $b_missing) {
        $only_merged_away = !($a_missing && !$a_merged) && !($b_missing && !$b_merged);
        return $only_merged_away;
    }

    return false;
}
