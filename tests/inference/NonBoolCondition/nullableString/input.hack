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

function string_t_condition(some_str_t $value): void {
    if ($value) {
        echo $value;
    }
}

function string_newtype_condition(newtype_str_t $value): void {
    if ($value) {
        echo $value;
    }
}
