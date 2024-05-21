final class A {
    private string $prev = "";

    public function getPrevious(string $current): string {
        $prev = $this->prev;
        $this->prev = $current;
        return $prev;
    }
}

function foo(): void {
    $a = new A();
    $a->getPrevious($_GET["a"]);
    echo $a->getPrevious("foo");
}
