interface Foo {
    public function doFoo(): int;
}

final class Bar implements Foo {
    public function doFoo(): int {
      print "Error\n";
      exit(1);
    }
}

final class Baz implements Foo {
    public function doFoo(): int {
        throw new \Exception("bad");
    }
}