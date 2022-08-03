class A {
    public string $taint = "";

    public function getTaint() : string {
        return $this->taint;
    }
}

class B extends A {
    public function __construct(string $taint) {
        $this->taint = $taint;
    }
}

$b = new B($_GET["bar"]);
echo $b->getTaint();