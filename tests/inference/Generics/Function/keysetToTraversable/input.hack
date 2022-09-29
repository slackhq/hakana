function foo(keyset<int> $keys): void {
    $first = takesGeneric($keys);
    if ($first is nonnull) {
        echo $first;
    }
}

function takesGeneric<T>(Traversable<T> $t): ?T {
    return C\first($t);
}