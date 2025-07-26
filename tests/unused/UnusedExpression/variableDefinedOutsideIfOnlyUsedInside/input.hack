function test_basic_case(): void {
    $x = rand(0, 1);
    if (rand() > 0) {
        echo $x;
    }
}

function test_should_not_trigger_used_outside(): void {
    $x = rand(0, 1);
    if (rand() > 0) {
        echo $x;
    }
    echo $x;
}

function test_should_not_trigger_no_if_blocks(): void {
    $x = rand(0, 1);
    echo $x;
}

function test_nested_if(): void {
    $x = rand(0, 1);
    if (rand() > 0) {
        if (rand() > 1) {
            echo $x;
        }
    }
}

function test_else_block(): void {
    $x = rand(0, 1);
    if (rand() > 0) {
        echo "false";
    } else {
        echo $x;
    }
}

function test_both_if_and_else_blocks(): void {
    $x = rand(0, 1);
    if (rand() > 0) {
        echo $x;
    } else {
        echo $x;
    }
}

function test_defined_inside_if(): void {
    if (rand() > 0) {
        $x = 5;
        echo $x;
    }
}

function test_foreach_optimization(vec<int> $ints): void {
    $x = rand(0, 1) ? 'a' : 'b';
    foreach ($ints as $item) {
        if (rand() > 0) {
            echo $x.$item;
        }
    }
}

function test_while_optimization(): void {
    $x = rand(0, 1);
    while (rand() > 0) {
        if (rand() > 1) {
            echo $x;
        }
    }
}

function test_call_with_pure_outputs(): void {
    $x = rand(0, 1) ? 'a' : 'b';
    if (rand() > 0) {
        echo $x;
    }
}

function test_pure(): void {
    $x = 'a';
    if (rand() > 0) {
        echo $x;
    }
}