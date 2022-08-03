class A {}

function bar(?A $a) : void {
    if (rand(0, 1) && (!$a || rand(0, 1))) {
        if ($a !== null) {}
    }
}