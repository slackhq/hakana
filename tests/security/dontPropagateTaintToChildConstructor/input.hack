class A {
    public function __construct(string $a) {}
}

class B extends A {
    public function __construct(string $a) {
        echo $a;
    }
}

new A($_GET["foo"]);