final class T {
    public bool $b = false;
}

function foo(
    ?T $t
): void {
    if (!$t) {
        $t = new T();
    } else if (rand(0, 1)) {
        //
    }

    if ($t->b) {}
}