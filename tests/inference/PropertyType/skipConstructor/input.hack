abstract class A {
    protected string $s;

    public function __construct() {
        $this->s = "hello";
    }
}

abstract class B extends A {}

final class C extends B {
    public function __construct()
    {
        parent::__construct();

        echo $this->s;
    }
}