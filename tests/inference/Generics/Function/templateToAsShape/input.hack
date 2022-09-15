function foo<T>(T $t): dict<arraykey, mixed> {
    return $t as dict<_, _>;
}

function bar<T>(T $t): dict<arraykey, mixed> {
    return $t as shape(...);
}