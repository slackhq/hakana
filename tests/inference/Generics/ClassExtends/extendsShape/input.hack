type foo = shape('a' => int, ...);
type foo_extended = shape('a' => int, 'b' => int, ...);

class Maker<T as foo> {
  public function __construct(T $args) {}
}

class ExtendedMaker extends Maker<foo_extended> {}
class FurtherExtendedMaker extends ExtendedMaker {}

function bar(): void {
  new FurtherExtendedMaker(shape('a' => 5, 'b' => 6));
}