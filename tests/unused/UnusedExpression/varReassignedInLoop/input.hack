function foo(): void {
    $a = 'hello';
    
    while (rand(0, 1)) {
        bar($a);

        if (rand(0, 1)) {
            // this reassignment is bad
            $a = 'goodbye';
            bar($a);
        }
    }
}

function bar(string $_s): void {}
