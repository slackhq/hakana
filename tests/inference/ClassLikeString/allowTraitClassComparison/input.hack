trait T {
    public function foo() : void {
        if (self::class === A::class) {}
        if (self::class !== A::class) {}
    }
}

class A {
    use T;
}

class B {
    use T;
}