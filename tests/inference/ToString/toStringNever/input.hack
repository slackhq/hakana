final class B{
    public function __toString() {
        throw new BadMethodCallException("bad");
    }
}
                