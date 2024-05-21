<<__ConsistentConstruct>>
abstract class A {
    public function getInstance() : A {
        return new static();
    }
}

final class AChild extends A {
    public function __construct() {}
}