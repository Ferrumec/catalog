document.querySelectorAll(".add-to-cart").forEach((link) => {
  link.addEventListener("click", function (e) {
    e.preventDefault();

    const url = this.href;

    fetch(url, {
      method: "POST",
      headers: {
        "Content-Type": "application/json",
      },
    }).then(() => {
      console.log("Logged:", url);
      // optional: navigate after logging
      // window.location.href = url;
    });
  });
});

document.querySelectorAll(".start-order").forEach((link) => {
  link.addEventListener("click", function (e) {
    e.preventDefault();

    const url = this.href;
    let productId = url.split("/").pop(); // extract product ID from URL

    // Add product id to cart before starting order
    fetch("/cart/add/" + productId).then(() => {
location.replace(url); // navigate to order page after adding to cart
    });
  });
});
