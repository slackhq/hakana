abstract class A {}

trait MyTrait {
	require extends A;
    
    public function foo(): void {
        if ($this is AGrandChild) {
            $this->bar();
        }
    }
}

class AChild extends A {
    use MyTrait;
    public function bar(): void {}
}

class AGrandChild extends AChild {}