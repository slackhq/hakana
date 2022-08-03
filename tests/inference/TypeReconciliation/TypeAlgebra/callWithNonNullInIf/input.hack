function sayHello(?int $a, ?int $b): void {
    if ($a === null && $b === null) {
        throw new \LogicException();
    }

    if ($a !== null) {
        takesInt($a);
    } else {
        takesInt($b);
    }
}

function takesInt(int $c) : void {}