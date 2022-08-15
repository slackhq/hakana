<<\Hakana\SecurityAnalysis\SpecializeInstance()>>
class User {
    public function __construct(public string $userId) {}
}

function bar(): User {
    return new User($_GET["user_id"]);
}

function calls_bar() {
    $bar = bar();
    echo $bar->userId;
}