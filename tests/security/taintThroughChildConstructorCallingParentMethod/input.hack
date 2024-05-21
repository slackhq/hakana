abstract class A {
    private string $taint;

    public function __construct($taint) {
        $this->taint = $taint;
    }

    public function getTaint() : string {
        return $this->taint;
    }
}

final class B extends A {}

final class C extends B {}

function foo(B $b) {
    echo $b->getTaint();
}

$c = new C($_GET["bar"]);

foo($c);