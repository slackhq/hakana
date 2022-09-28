function foo(KeyedContainer<string, mixed> $dict): shape('a' => string, ...) {
    return $dict as shape('a' => string, ...);
}