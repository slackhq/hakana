final class A {
    public int $x = 0;
}

function foo<T as A>(inout T $obj): void {
    $obj->x = 1;
}