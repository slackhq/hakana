function nullable_string_condition(?string $value): void {
    if ($value) {
        echo $value;
    }
}

function string_condition(string $value): void {
    if ($value) {
        echo $value;
    }
}
