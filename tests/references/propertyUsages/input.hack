class User {
    public string $name;
    private int $age;

    public function __construct(string $name, int $age) {
        $this->name = $name;
        $this->age = $age;
    }

    public function getName(): string {
        return $this->name;
    }

    public function getAge(): int {
        return $this->age;
    }

    public function setName(string $name): void {
        $this->name = $name;
    }
}

function test(): void {
    $user = new User("Alice", 30);
    $userName = $user->name;
    $user->name = "Bob";
}
