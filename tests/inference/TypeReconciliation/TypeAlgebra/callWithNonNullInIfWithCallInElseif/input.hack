function sayHello(?int $a, ?int $b): void {
    if ($a === null && $b === null) {
        throw new \LogicException();
    }

    if ($a !== null) {
        takesInt($a);
    } else if (rand(0, 1)) {
        takesInt($b);
    }
}

function takesInt(int $c) : void {}