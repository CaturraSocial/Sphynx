
/**
 * Handles bad request errors by logging the error and
 * sending a response to the client with a 400 status code
 * and a simple HTML page with a "Bad Request" message.
 *
 * @param {string} err - The error message to log and send to the client.
 * @param {http.ServerResponse} response - The response object for the request.
 */
const badRequestError = (err, response) => {
    console.error(`Bad Request Error: ${err}`);
    response.writeHead(400, { 'Content-Type': 'text/html' });
    response.end(`400 Bad Request: ${err}`, 'utf-8');
}

/**
 * Handles server errors by logging the error and
 * sending a response to the client with a 500 status code
 * and a simple HTML page with a "Server Error" message.
 * @param {Error} err - The error that caused the server error.
 * @param {http.ServerResponse} response - The response object to send a response to the client with.
 */
const serverError = (err, response) => {
    console.error(err);
    response.writeHead(500, { 'Content-Type': 'text/html' });
    response.end('500 Server Error');
}

/**
 * Handles 404 Not Found errors by logging the error and
 * sending a response to the client with a 404 status code
 * and a simple HTML page with a "Not Found" message.
 * 
 * @param {url.URL} requestUrl - The URL of the request that was made.
 * @param {http.ServerResponse} response - The response object for the request.
 */
const notFoundError = (requestUrl, response) => {
    console.error(`Path ${requestUrl.pathname} not found`);
    response.writeHead(404, { 'Content-Type': 'text/html' });
    response.end('404 Not Found', 'utf-8');
}

module.exports = { badRequestError, serverError, notFoundError };