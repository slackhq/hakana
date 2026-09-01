function foo(int $i, string $alias) : void {
    $i++;
    echo $alias !== "" ? $alias : $i;
    echo $i;
}
