function redirect(<<Hakana\SecurityAnalysis\Sink('RedirectUri')>> string $url): void {}
redirect('/' . (string)HH\global_get('_GET')['path']);
