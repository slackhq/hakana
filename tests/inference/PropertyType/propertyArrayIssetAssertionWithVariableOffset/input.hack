function bar(string $s): void { }

class A {
    public dict<string, string> $a = dict[];

    private function foo(): void {
        $b = "hello";

        if (!isset($this->a[$b])) {
            return;
        }

        bar($this->a[$b]);
    }
}