final class A {
    private string $taint = "";

    public function __construct(string $taint) {
        $this->taint = $taint;
    }

    public function getTaint() : string {
        return $this->taint;
    }
}

$a = new A(HH\global_get('_GET')["bar"]);
echo $a->getTaint();