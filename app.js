const http = require('http');
const url  = require('url');
const proxy = require('./proxy');
const proxyErrors = require('./proxyErrors');
const config = require('./config.json');

const CONFIG = {
    ip: config.IP || '127.0.0.1',
    port: config.PORT || 3000
}

const httpRequestHandler = (request, response) => {
    console.log(`Request received: ${request.url}`);
    let {serverError, notFoundError} = proxyErrors;
    const requestUrl = url.parse(request.url, true);  
    switch (requestUrl.pathname){   
    case '/':
        // response.writeHead(200, { 'Content-Type': 'text/html' });
        // response.write('<html><body><h1>Proxy Server</h1></body></html>');
        // response.end();
        try {
            proxy.proxyHandler(requestUrl, response);
        }
        catch(e) {
            serverError(e, response);
        }
        break;    
    case '/image':
        try {
            proxy.imageProxyHandler(requestUrl, response);
        }
        catch(e) {
            serverError(e, response);
        }
        break;
    case '/audio':
        try {
            proxy.audioProxyHandler(requestUrl, response);
        }
        catch(e) {
            serverError(e, response);
        }
        break;
    case '/video':
        try {
            proxy.videoProxyHandler(requestUrl, response);
        }
        catch(e) {
            serverError(e, response);
        }
        break;
    default:
        notFoundError(requestUrl, response);
        break;  
    }
}

http.createServer(httpRequestHandler).listen(CONFIG.port, CONFIG.ip);
console.log(`Server running at http://${CONFIG.ip}:${CONFIG.port}/`);

module.exports = { CONFIG }