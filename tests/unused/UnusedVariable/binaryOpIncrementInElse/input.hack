function foo(int $i, string $alias) : void {
    echo $alias ?: $i++;
    echo $i;
}