abstract class A {}

trait MyTrait {
	require extends A;
    
    public function foo(): void {
        if ($this is AGrandChild) {
            $this->bar();
        }
    }
}

abstract class AChild extends A {
    use MyTrait;
    public function bar(): void {}
}

final class AGrandChild extends AChild {}