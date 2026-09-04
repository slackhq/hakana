abstract class Base {
    public function helper(): void {}
}

trait T {
    public function go(): void {
        $this->helper();
    }
}

final class Impl extends Base {
    use T;
}

<<__EntryPoint>>
function main(): void {
    (new Impl())->go();
}
