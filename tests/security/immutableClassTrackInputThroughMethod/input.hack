class A {
    private string $taint = "";

    public function __construct(string $taint) {
        $this->taint = $taint;
    }

    public function getTaint() : string {
        return $this->taint;
    }
}

$a = new A($_GET["bar"]);
echo $a->getTaint();