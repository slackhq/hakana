function foo(iterable $iterable) : void {
    if ($iterable is KeyedContainer<_, _>) {}
    if ($iterable is \Traversable) {}
}