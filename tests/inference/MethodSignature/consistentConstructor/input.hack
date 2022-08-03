<<__ConsistentConstruct>>
class A {
    public function getInstance() : A {
        return new static();
    }
}

class AChild extends A {
    public function __construct() {}
}