abstract class A {
    public dict<arraykey, mixed> $arr;
}

final class B extends A {
    protected function __construct(public dict<arraykey, mixed> $arr){}
}