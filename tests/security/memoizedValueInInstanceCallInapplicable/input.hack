final class A {
    private string $prev = "";

    public function getPrevious(string $current): string {
        $prev = $this->prev;
        $this->prev = $current;
        return $prev;
    }
}

function foo(): void {
    $a1 = new A();
    $a1->getPrevious($_GET["a"]);
    $a1 = new A();
    echo $a1->getPrevious("foo");
}
