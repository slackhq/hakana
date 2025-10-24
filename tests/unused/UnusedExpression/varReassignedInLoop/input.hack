function foo(): void {
    $a = 'hello';
    $b = 'hello again';
    $c = 'hello a third time';
    
    while (rand(0, 1)) {
        bar($a);
        bar($c);

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

        if (rand(0, 1)) {
            // this reassignment is fine because we're not referencing it after within this block
            $c = 'goodbye';
        }

        // this is ok
        bar($b);
    }
}

function bar(string $_s): void {}