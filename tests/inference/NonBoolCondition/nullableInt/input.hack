function nullable_int_condition(?int $value): void {
    if ($value) {
        echo $value;
    }

    if (!$value) {
        echo $value;
    }
}

function int_type(some_int_t $t): void {
    if ($t) {
        echo "value";
    }
}

function int_newtype(newtype_int_t $t): void {
    if ($t) {
        echo "value";
    }
}
