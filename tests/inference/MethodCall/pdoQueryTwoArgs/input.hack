$pdo = new PDO("test");
$pdo->query("SELECT * FROM projects", PDO::FETCH_NAMED);