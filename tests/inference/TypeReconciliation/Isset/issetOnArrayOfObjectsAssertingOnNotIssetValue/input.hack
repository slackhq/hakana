final class A {
    public ?string $name = null;
}

function foo(int $i, dict<int, A> $tokens) : void {
    $tokens = dict[];

    if (!isset($tokens[$i])) {
        if (rand(0, 1) !== 0) {
            if (rand(0, 1) !== 0) {
                $tokens[$i] = new A();
            } else {
                return;
            }
        } else {
            return;
        }
    }

    echo $tokens[$i]->name;
}
