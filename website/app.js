// The site works without JavaScript. Only enhance local, same-page navigation.
document.querySelectorAll('a[href="#"]').forEach((link) => {
  link.addEventListener("click", (event) => {
    event.preventDefault();
    document.getElementById("hero-title").scrollIntoView({ block: "center" });
    history.replaceState(null, "", location.pathname);
  });
});
