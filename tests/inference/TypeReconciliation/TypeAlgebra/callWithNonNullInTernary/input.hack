function sayHello(?int $a, ?int $b): void {
    if ($a === null && $b === null) {
        throw new \LogicException();
    }

    takesInt($a !== null ? $a : $b);
}

function takesInt(int $c) : void {}