class A {
    private string $taint = "";

    public function __construct(string $taint) {
        $this->taint = $taint;
    }

    public function getTaint() : string {
        return $this->taint;
    }
}

$b = new A($_GET["bar"]);
$a = new A("bar");
echo $a->getTaint();