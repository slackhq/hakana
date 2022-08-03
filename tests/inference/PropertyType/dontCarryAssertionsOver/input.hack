class A
{
    private string $network;

    public function __construct(string $s)
    {
        $this->network = $s;
        $this->firstCheck();
        $this->secondCheck();
    }

    public function firstCheck(): void
    {
        if ($this->network === "x") {
            return;
        }
    }

    public function secondCheck(): void
    {
        if ($this->network === "x") {
            return;
        }
    }
}