function Foo(mixed $width, mixed $height) : num {
    if (!($width is int || $width is float) || !($height is int || $height is float)) {
        throw new RuntimeException("bad");
    }

    return $width / $height;
}