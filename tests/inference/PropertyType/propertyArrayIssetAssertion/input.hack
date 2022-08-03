function bar(string $s): void { }

class A {
    public dict<string, string> $a = dict[];

    private function foo(): void {
        if (isset($this->a["hello"])) {
            bar($this->a["hello"]);
        }
    }
}