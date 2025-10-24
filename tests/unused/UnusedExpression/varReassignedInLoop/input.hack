function foo(): void {
    $a = 'hello';
    $b = 'hello again';
    
    while (rand(0, 1)) {
        bar($a);

        if (rand(0, 1)) {
            // this reassignment is bad
            $a = 'goodbye';
            bar($a);
        }

        if (rand(0, 1)) {
            // this reassignment is ok as variable was not previously used in loop
            $b = 'goodbye again';
            bar($b);
        }

        // this is ok
        bar($b);
    }
}

function bar(string $_s): void {}