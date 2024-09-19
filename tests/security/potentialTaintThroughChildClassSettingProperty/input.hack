abstract class A {
    public string $taint = "";

    public function getTaint() : string {
        return $this->taint;
    }
}

final class B extends A {
    public function __construct(string $taint) {
        $this->taint = $taint;
    }
}

$b = new B(HH\global_get('_GET')["bar"]);
echo $b->getTaint();