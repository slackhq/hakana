function foo(vec<string> $v): void {
    if (!$v is vec<_>) {
        return;
    }

    if (C\count($v) > 10) {
        // do something
    }
}