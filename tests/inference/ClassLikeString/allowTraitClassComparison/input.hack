trait T {
    public function foo() : void {
        if (self::class === A::class) {}
        if (self::class !== A::class) {}
    }
}

final class A {
    use T;
}

final class B {
    use T;
}