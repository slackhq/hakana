class SomeClass {
    private ?int $int;

    public function __construct() {
        $this->int = 1;
    }

    public function getInt(): ?int {
        return $this->int;
    }
}

function printInt(int $int): void {
    echo $int;
}

$obj = new SomeClass();

if ($obj->getInt()) {
    printInt($obj->getInt());
}