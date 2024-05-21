abstract class Foobar {
    public function doIt(): void {
        $this->inner();
    }

    abstract protected function inner(): void;
}

final class MyFooBar extends Foobar {
    protected function inner(): void {
        // Do nothing
    }
}

<<__EntryPoint>>
function foo(): void {
    $myFooBar = new MyFooBar();
    $myFooBar->doIt();
}
