location /api/ {
    proxy_pass http://elevation:3000/;
    rewrite ^/api(/.*)$ $1 break;
}