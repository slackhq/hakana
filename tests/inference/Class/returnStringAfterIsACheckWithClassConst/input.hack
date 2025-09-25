final class Foo{}
function bar(string $maybeBaz) : string {
  if (!is_a($maybeBaz, nameof Foo, true)) {
    throw new Exception("not Foo");
  }
  return $maybeBaz;
}