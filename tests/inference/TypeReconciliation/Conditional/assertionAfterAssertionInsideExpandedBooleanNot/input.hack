final class A {}

function bar(?A $a) : void {
    if (rand(0, 1) !== 0 && ($a === null || rand(0, 1) !== 0)) {
        if ($a !== null) {}
    }
}