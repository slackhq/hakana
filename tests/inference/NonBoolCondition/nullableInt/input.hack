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


function foo(bool $input, ?int $nullable_x, ?int $nullable_y): void {
    if ($input && (bool)$nullable_x || !(bool)$nullable_y) {
        $foo = (bool)$nullable_x;
    }
}
