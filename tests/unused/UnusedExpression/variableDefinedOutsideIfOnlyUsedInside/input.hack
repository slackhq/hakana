function test_basic_case(): void {
    $x = 5;
    if (rand() > 0) {
        echo $x;
    }
}

function test_should_not_trigger_used_outside(): void {
    $x = 5;
    if (rand() > 0) {
        echo $x;
    }
    echo $x;
}

function test_should_not_trigger_no_if_blocks(): void {
    $x = 5;
    echo $x;
}

function test_nested_if(): void {
    $x = 5;
    if (rand() > 0) {
        if (rand() > 1) {
            echo $x;
        }
    }
}

function test_else_block(): void {
    $x = 5;
    if (rand() > 0) {
        echo "false";
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