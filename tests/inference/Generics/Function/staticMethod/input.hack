abstract class A {}
final class B extends A {
  public function bar(): void {}
}

final class Config {
  public static function get<reify T as A>(): T {
    throw new \Exception("bad");
  }
}

function foo(): void {
  Config::get<B>()->bar();
}