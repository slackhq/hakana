final class A {
    public int $a = 0;
    public int $b = 1;

    public function setPhpVersion(string $version): void {
        list($a, $b) = explode(".", $version);

        $this->a = (int) $a;
        $this->b = (int) $b;
    }
}
