final class Foo {
  const type TConstructorShape = shape(
    ?'a' => ?string,
    ?'b' => ?int,
  );

  public function __construct(private ?string $a, private ?int $b)[] {}

  public static function fromShape(self::TConstructorShape $shape)[]: Foo {
    return new Foo(
        Shapes::idx($shape, 'a'),
        Shapes::idx($shape, 'b')
    );
  }
}