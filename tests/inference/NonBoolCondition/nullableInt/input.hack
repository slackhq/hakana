function nullable_int_condition(?int $value): void {
    if ($value) {
        echo $value;
    }

    if (!$value) {
        echo $value;
    }
}
