function foo(): void {
    try {
        while (true) {
            bar();
            if (rand(0, 1)) {
                break;
            }
        }
        echo "got here";
    } catch (Exception $e) {
        return;
    }
    echo "also got here";
}

function bar(): void {}

function baz(): void {
    try {
        while (true) {
            bar();
            while (true) {
                if (rand(0, 1)) {
                    break;
                }
            }
        }
        echo "got here";
    } catch (Exception $e) {
        return;
    }
    echo "also got here";
}