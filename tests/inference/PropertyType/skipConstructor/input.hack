class A {
    protected string $s;

    public function __construct() {
        $this->s = "hello";
    }
}

class B extends A {}

class C extends B {
    public function __construct()
    {
        parent::__construct();

        echo $this->s;
    }
}