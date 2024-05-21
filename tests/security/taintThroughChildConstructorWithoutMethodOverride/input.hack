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


$b = new B($_GET["bar"]);
echo $b->getTaint();