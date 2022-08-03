abstract class Foo {}

abstract class FooChild extends Foo {}

abstract class AbstractTestCase {
    abstract public function createFoo(): Foo;
}

abstract class AbstractChildTestCase extends AbstractTestCase {
    abstract public function createFoo(): FooChild;

    public function testFoo(): FooChild {
        return $this->createFoo();
    }
}