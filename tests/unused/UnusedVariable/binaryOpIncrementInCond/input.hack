function foo(int $i, string $alias) : void {
    echo $i++ ?: $alias;
    echo $i;
}