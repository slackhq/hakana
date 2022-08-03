class A {
    private $taint;

    public function __construct($taint) {
        $this->taint = $taint;
    }

    public function getTaint() : string {
        return $this->taint;
    }
}

class B extends A {}

class C extends B {}

$c = new C($_GET["bar"]);

function foo(B $b) {
    echo $b->getTaint();
}