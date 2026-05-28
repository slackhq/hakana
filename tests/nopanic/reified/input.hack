abstract class A {
    public static function foo(): string {
        return "test";
    }

    public function work(): void {}
}

final class B<reify TParam as A> {
    public function run(): void {
        $t = new TParam();
        $t->work();
    }

    public function call(): string {
        return TParam::foo();
    }
}
