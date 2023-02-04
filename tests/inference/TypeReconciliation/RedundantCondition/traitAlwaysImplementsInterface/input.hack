interface I {}

trait T {
    require implements I;

    public function foo(): void {
        if (!$this is I) {
            return;
        }
    }
}