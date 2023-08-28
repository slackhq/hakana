class A {
    private string $taint;

    public function __construct($taint) {
        $this->taint = $taint;
    }

    public function getTaint() : string {
        return $this->taint;
    }
}

class B extends A {}

class C extends B {}

function foo(A $a) {
    echo $a->getTaint();
}

$c = new C($_GET["bar"]);

foo($c);